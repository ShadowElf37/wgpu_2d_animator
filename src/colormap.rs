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
    pub fn label(self) -> &'static str {
        match self {
            Colormap::Heat      => "heat",
            Colormap::Inferno   => "inferno",
            Colormap::Viridis   => "viridis",
            Colormap::RdBu      => "rdbu",
            Colormap::Grayscale => "grayscale",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Colormap::Heat      => Colormap::Inferno,
            Colormap::Inferno   => Colormap::Viridis,
            Colormap::Viridis   => Colormap::RdBu,
            Colormap::RdBu      => Colormap::Grayscale,
            Colormap::Grayscale => Colormap::Heat,
        }
    }

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

// Redesigned heat: black → dark red → red → orange → yellow.
// No white endpoint so orange/yellow remain visible at peak values.
// Orange spans ~0.55–0.85 (was crammed into 0.60–0.80 before).
static HEAT: &[(f32, [f32; 3])] = &[
    (0.00, [0.000, 0.000, 0.000]),   // black
    (0.25, [0.500, 0.000, 0.000]),   // dark red
    (0.45, [1.000, 0.050, 0.000]),   // red
    (0.62, [1.000, 0.380, 0.000]),   // orange-red
    (0.78, [1.000, 0.680, 0.000]),   // orange
    (0.90, [1.000, 0.930, 0.000]),   // yellow
    (1.00, [1.000, 1.000, 0.550]),   // pale yellow (not white)
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

// Sampled from the actual matplotlib viridis LUT (_cm_listed.py) at 16 equally-spaced
// indices (0, 17, 34, 51, 68, 85, 102, 119, 136, 153, 170, 187, 204, 221, 238, 255).
// Previous version had red/green too high in the purple range, making it look washed out.
static VIRIDIS: &[(f32, [f32; 3])] = &[
    (0.000, [0.267, 0.005, 0.329]),  // #440154 dark purple
    (0.067, [0.276, 0.100, 0.422]),  // #46197c purple (green was too high before)
    (0.133, [0.244, 0.180, 0.487]),  // #3e2e7c blue-purple
    (0.200, [0.207, 0.258, 0.537]),  // #354284 slate blue
    (0.267, [0.170, 0.327, 0.553]),  // #2b538d muted blue
    (0.333, [0.135, 0.393, 0.557]),  // #22648e steel blue
    (0.400, [0.106, 0.453, 0.547]),  // #1b748b teal-blue
    (0.467, [0.110, 0.512, 0.525]),  // #1d8384 teal
    (0.533, [0.146, 0.571, 0.494]),  // #25927e teal-green
    (0.600, [0.220, 0.629, 0.447]),  // #38a172 medium green
    (0.667, [0.326, 0.683, 0.389]),  // #53ae63 green
    (0.733, [0.441, 0.730, 0.314]),  // #71ba50 yellow-green
    (0.800, [0.555, 0.769, 0.227]),  // #8ec43a chartreuse
    (0.867, [0.669, 0.806, 0.140]),  // #abcd24 lime
    (0.933, [0.798, 0.857, 0.095]),  // #ccdb18 bright lime
    (1.000, [0.993, 0.906, 0.144]),  // #fde725 yellow
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
