// data.wgsl
// Full-screen quad that reads a 2D R32Float data texture and maps each
// value through the heat colormap (black → red → white).

// ── Uniforms ─────────────────────────────────────────────────────────────────
struct Uniforms {
    vmin: f32,
    vmax: f32,
    _pad: vec2<f32>,   // pad to 16 bytes for uniform buffer alignment
};

@group(0) @binding(0) var<uniform> u:        Uniforms;
@group(0) @binding(1) var          data_tex: texture_2d<f32>;

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

// ── Colormap ──────────────────────────────────────────────────────────────────
// Heat: black → red → white via three piecewise-linear ramps.
//   t ∈ [0.0, 0.5] : r ramps 0→1, g and b stay 0   (black → red)
//   t ∈ [0.5, 1.0] : g and b ramp 0→1, r stays 1   (red → white)
fn heat(t: f32) -> vec3<f32> {
    return vec3<f32>(
        clamp(t * 2.0,       0.0, 1.0),
        clamp(t * 2.0 - 1.0, 0.0, 1.0),
        clamp(t * 2.0 - 1.0, 0.0, 1.0),
    );
}

// ── Fragment shader ───────────────────────────────────────────────────────────
// textureLoad (integer coords) instead of textureSample: R32Float is
// non-filterable on Metal/Vulkan without an optional feature flag.
// Nearest-neighbour is correct for now; interpolation is added at M6.
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
