// data.wgsl
// Full-screen quad that reads a 2D R32Float data texture and maps each
// value through a 1D RGBA8 colormap LUT (256 entries, filterable).

// ── Uniforms ──────────────────────────────────────────────────────────────────
struct Uniforms {
    vmin:        f32,
    vmax:        f32,
    interp_mode: u32,        // 0 = nearest, 1 = bilinear, 2 = bicubic (Catmull-Rom)
    channels:    u32,        // 1 = scalar (LUT path), 3 = RGB passthrough
    pan:         vec2<f32>,  // view centre offset in data-UV space
    zoom:        f32,        // scale factor (> 1 = zoomed in)
    _pad2:       f32,
};

@group(0) @binding(0) var<uniform> u:         Uniforms;
@group(0) @binding(1) var          data_tex:  texture_2d<f32>;
@group(0) @binding(2) var          cmap_tex:  texture_1d<f32>;
@group(0) @binding(3) var          cmap_samp: sampler;

// ── Vertex shader ─────────────────────────────────────────────────────────────
// Generates a full-screen quad from vertex index alone (no vertex buffer).
// Topology: TriangleList, draw(0..6).
//
// Two triangles that tile the clip-space square [-1,1]²:
//   Triangle 0:  vi 0,1,2  →  bot-left, bot-right, top-left
//   Triangle 1:  vi 3,4,5  →  bot-right, top-right, top-left
//
// UV convention: (0,0) = top-left of data, (1,1) = bottom-right.
// The Y flip between clip (+y up) and UV (+y down) is handled by mapping
// clip y=-1 to UV y=1 and clip y=+1 to UV y=0.

struct VertOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertOut {
    //                      clip xy          UV
    var pos = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),   // tri 0 bot-left
        vec2<f32>( 1.0, -1.0),   // tri 0 bot-right
        vec2<f32>(-1.0,  1.0),   // tri 0 top-left
        vec2<f32>( 1.0, -1.0),   // tri 1 bot-right
        vec2<f32>( 1.0,  1.0),   // tri 1 top-right
        vec2<f32>(-1.0,  1.0),   // tri 1 top-left
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),     // bot-left  → UV (0,1)
        vec2<f32>(1.0, 1.0),     // bot-right → UV (1,1)
        vec2<f32>(0.0, 0.0),     // top-left  → UV (0,0)
        vec2<f32>(1.0, 1.0),     // bot-right → UV (1,1)
        vec2<f32>(1.0, 0.0),     // top-right → UV (1,0)
        vec2<f32>(0.0, 0.0),     // top-left  → UV (0,0)
    );
    var out: VertOut;
    out.clip_pos = vec4<f32>(pos[vi], 0.0, 1.0);
    out.uv       = uvs[vi];
    return out;
}

// ── Sampling helpers ──────────────────────────────────────────────────────────
// R32Float is non-filterable on Metal/Vulkan without an optional GPU feature,
// so all three modes use textureLoad (integer coords) and interpolate manually.

fn tex_load(coord: vec2<i32>, dims: vec2<i32>) -> f32 {
    return textureLoad(data_tex, clamp(coord, vec2<i32>(0), dims - vec2<i32>(1)), 0).r;
}

fn sample_nearest(uv: vec2<f32>, dims: vec2<i32>) -> f32 {
    return tex_load(vec2<i32>(uv * vec2<f32>(dims)), dims);
}

fn sample_linear(uv: vec2<f32>, dims: vec2<i32>) -> f32 {
    // Map UV to pixel-centre space, isolate integer and fractional parts.
    let p   = uv * vec2<f32>(dims) - 0.5;
    let i   = vec2<i32>(floor(p));
    let f   = p - floor(p);
    let c00 = tex_load(i,                   dims);
    let c10 = tex_load(i + vec2<i32>(1, 0), dims);
    let c01 = tex_load(i + vec2<i32>(0, 1), dims);
    let c11 = tex_load(i + vec2<i32>(1, 1), dims);
    return mix(mix(c00, c10, f.x), mix(c01, c11, f.x), f.y);
}

// Catmull-Rom cubic kernel (α = -0.5).
fn cubic_w(t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    if t < 1.0 {
        return 1.5 * t3 - 2.5 * t2 + 1.0;
    } else if t < 2.0 {
        return -0.5 * t3 + 2.5 * t2 - 4.0 * t + 2.0;
    }
    return 0.0;
}

fn sample_bicubic(uv: vec2<f32>, dims: vec2<i32>) -> f32 {
    let p = uv * vec2<f32>(dims) - 0.5;
    let i = vec2<i32>(floor(p));
    let f = p - floor(p);
    var result = 0.0;
    for (var jj: i32 = -1; jj <= 2; jj = jj + 1) {
        let wy = cubic_w(abs(f.y - f32(jj)));
        for (var ii: i32 = -1; ii <= 2; ii = ii + 1) {
            let wx = cubic_w(abs(f.x - f32(ii)));
            result = result + wx * wy * tex_load(i + vec2<i32>(ii, jj), dims);
        }
    }
    return result;
}

// ── Fragment shader ───────────────────────────────────────────────────────────
@fragment
fn fs_main(in: VertOut) -> @location(0) vec4<f32> {
    // Apply zoom/pan: transform screen UV → data UV.
    // zoom > 1 magnifies; pan offsets the view centre in data-UV space.
    let data_uv = (in.uv - vec2<f32>(0.5) - u.pan) / u.zoom + vec2<f32>(0.5);

    // Out-of-bounds pixels show the clear colour (dark blue-grey).
    if data_uv.x < 0.0 || data_uv.x > 1.0 || data_uv.y < 0.0 || data_uv.y > 1.0 {
        return vec4<f32>(0.05, 0.05, 0.12, 1.0);
    }

    let dims = vec2<i32>(textureDimensions(data_tex));

    // RGB passthrough: data texture is Rgba32Float, return colour directly.
    if u.channels == 3u {
        let coord = clamp(vec2<i32>(data_uv * vec2<f32>(dims)), vec2<i32>(0), dims - vec2<i32>(1));
        let col = textureLoad(data_tex, coord, 0);
        return vec4<f32>(col.rgb, 1.0);
    }

    // Scalar → colormap LUT path.
    var raw: f32;
    if u.interp_mode == 1u {
        raw = sample_linear(data_uv, dims);
    } else if u.interp_mode == 2u {
        raw = sample_bicubic(data_uv, dims);
    } else {
        raw = sample_nearest(data_uv, dims);
    }
    // NaN/inf: map inf→1 (colormap max), -inf→0, NaN→0 (colormap min).
    var t: f32;
    if raw != raw {          // NaN check (NaN != NaN is always true in IEEE 754)
        t = 0.0;
    } else {
        t = clamp((raw - u.vmin) / (u.vmax - u.vmin), 0.0, 1.0);
    }
    let col = textureSample(cmap_tex, cmap_samp, t);
    return vec4<f32>(col.rgb, 1.0);
}
