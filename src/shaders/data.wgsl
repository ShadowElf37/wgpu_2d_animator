// data.wgsl
// Full-screen quad that reads a 2D R32Float data texture and maps each
// value through the heat colormap (black → red → white).

// ── Uniforms ─────────────────────────────────────────────────────────────────
// Padded to 16 bytes so the struct satisfies wgpu uniform buffer alignment
// on every backend.  vmin/vmax define the data range mapped to [0, 1].
struct Uniforms {
    vmin: f32,
    vmax: f32,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u:        Uniforms;
@group(0) @binding(1) var          data_tex: texture_2d<f32>;

// ── Vertex shader ─────────────────────────────────────────────────────────────
// Generates a full-screen quad from vertex index alone (no vertex buffer).
// Topology: TriangleStrip, draw(0..4).
//
//  vi=0  top-left   clip(-1,+1)  UV(0,0)
//  vi=1  top-right  clip(+1,+1)  UV(1,0)
//  vi=2  bot-left   clip(-1,-1)  UV(0,1)
//  vi=3  bot-right  clip(+1,-1)  UV(1,1)
//
// The Y flip between clip space (+y up) and UV space (+y down) is handled
// by pairing clip +1 with UV 0 and clip -1 with UV 1 on the Y axis.

struct VertOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertOut {
    var pos = array<vec2<f32>, 4>(
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
    );
    var uvs = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
    );
    var out: VertOut;
    out.clip_pos = vec4<f32>(pos[vi], 0.0, 1.0);
    out.uv       = uvs[vi];
    return out;
}

// ── Colormap ──────────────────────────────────────────────────────────────────
// Heat: three piecewise-linear ramps on r, g, b.
//   t ∈ [0.0, 0.5] : black → red   (r ramps 0→1, g/b stay 0)
//   t ∈ [0.5, 1.0] : red   → white (g/b ramp 0→1, r stays 1)
fn heat(t: f32) -> vec3<f32> {
    return vec3<f32>(
        clamp(t * 2.0,       0.0, 1.0),
        clamp(t * 2.0 - 1.0, 0.0, 1.0),
        clamp(t * 2.0 - 1.0, 0.0, 1.0),
    );
}

// ── Fragment shader ───────────────────────────────────────────────────────────
// Uses textureLoad (integer coordinates) instead of textureSample because
// R32Float is non-filterable on Metal and Vulkan without an optional feature.
// Nearest-neighbour is correct for M2; interpolation is added at M6.
@fragment
fn fs_main(in: VertOut) -> @location(0) vec4<f32> {
    let dims  = vec2<i32>(textureDimensions(data_tex));
    let coord = clamp(
        vec2<i32>(in.uv * vec2<f32>(dims)),
        vec2<i32>(0, 0),
        dims - vec2<i32>(1, 1),
    );
    let raw = textureLoad(data_tex, coord, 0).r;
    let t   = clamp((raw - u.vmin) / (u.vmax - u.vmin), 0.0, 1.0);
    return vec4<f32>(heat(t), 1.0);
}
