/// Five built-in colormaps, each baked into a 256-entry RGBA8 LUT.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Colormap {
    #[default]
    Heat,
    Inferno,
    Viridis,
    RdBu,
    Grayscale,
}

impl Colormap {
    /// Returns 1024 bytes (256 × RGBA8) ready to upload as a 1D texture.
    pub fn lut_rgba8(self) -> Vec<u8> {
        let stops: &[(f32, [f32; 3])] = match self {
            Colormap::Heat      => HEAT,
            Colormap::Inferno   => INFERNO,
            Colormap::Viridis   => VIRIDIS,
            Colormap::RdBu      => RDBU,
            Colormap::Grayscale => GRAYSCALE,
        };
        build_lut(stops)
    }
}

// ── Colormap stop tables ──────────────────────────────────────────────────────
// Each entry: (normalised position t, [R, G, B]) in linear float.
// Positions must be in ascending order; first ≤ 0 and last ≥ 1.

static HEAT: &[(f32, [f32; 3])] = &[
    (0.00, [0.000, 0.000, 0.000]),   // black
    (0.33, [1.000, 0.000, 0.000]),   // red
    (0.60, [1.000, 0.450, 0.000]),   // orange
    (0.80, [1.000, 1.000, 0.000]),   // yellow
    (1.00, [1.000, 1.000, 1.000]),   // white
];

static GRAYSCALE: &[(f32, [f32; 3])] = &[
    (0.0, [0.0, 0.0, 0.0]),
    (1.0, [1.0, 1.0, 1.0]),
];

// Approximated from matplotlib's Inferno (perceptually uniform, dark→bright).
static INFERNO: &[(f32, [f32; 3])] = &[
    (0.00, [0.000, 0.000, 0.004]),
    (0.25, [0.316, 0.032, 0.412]),
    (0.50, [0.735, 0.130, 0.239]),
    (0.75, [0.941, 0.534, 0.058]),
    (1.00, [0.988, 0.998, 0.645]),
];

// Approximated from matplotlib's Viridis (perceptually uniform, blue→yellow).
static VIRIDIS: &[(f32, [f32; 3])] = &[
    (0.00, [0.267, 0.005, 0.329]),
    (0.25, [0.229, 0.322, 0.545]),
    (0.50, [0.128, 0.566, 0.551]),
    (0.75, [0.370, 0.788, 0.384]),
    (1.00, [0.993, 0.906, 0.144]),
];

// Approximated from matplotlib's RdBu (diverging, red→neutral→blue).
// Useful for signed fields (e.g. Hz).
static RDBU: &[(f32, [f32; 3])] = &[
    (0.0, [0.647, 0.000, 0.149]),
    (0.1, [0.843, 0.188, 0.153]),
    (0.2, [0.957, 0.427, 0.263]),
    (0.3, [0.992, 0.682, 0.518]),
    (0.4, [0.996, 0.878, 0.824]),
    (0.5, [0.969, 0.969, 0.969]),
    (0.6, [0.820, 0.898, 0.941]),
    (0.7, [0.573, 0.773, 0.871]),
    (0.8, [0.263, 0.576, 0.765]),
    (0.9, [0.129, 0.400, 0.675]),
    (1.0, [0.020, 0.188, 0.380]),
];

// ── LUT builder ───────────────────────────────────────────────────────────────

fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

fn build_lut(stops: &[(f32, [f32; 3])]) -> Vec<u8> {
    let mut lut = Vec::with_capacity(256 * 4);
    for i in 0u32..256 {
        let t = i as f32 / 255.0;
        // Index of first stop whose position > t.
        let hi_idx = stops.partition_point(|s| s.0 <= t);
        let lo_idx = hi_idx.saturating_sub(1);
        let hi_idx = hi_idx.min(stops.len() - 1);
        let lo = &stops[lo_idx];
        let hi = &stops[hi_idx];
        let span = hi.0 - lo.0;
        let alpha = if span > 0.0 { ((t - lo.0) / span).clamp(0.0, 1.0) } else { 0.0 };
        for k in 0..3 {
            let v = lerp(lo.1[k], hi.1[k], alpha).clamp(0.0, 1.0);
            lut.push((v * 255.0 + 0.5) as u8);
        }
        lut.push(255); // A = opaque
    }
    lut
}
