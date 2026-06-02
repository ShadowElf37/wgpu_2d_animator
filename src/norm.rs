#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum NormMode {
    #[default]
    Global,
    PerFrame,
    Percentile,
    Fixed,
}

impl NormMode {
    pub fn label(self) -> &'static str {
        match self {
            NormMode::Global      => "global",
            NormMode::PerFrame    => "per-frame",
            NormMode::Percentile  => "percentile 2–98",
            NormMode::Fixed       => "fixed",
        }
    }

    pub fn next(self) -> Self {
        match self {
            NormMode::Global      => NormMode::PerFrame,
            NormMode::PerFrame    => NormMode::Percentile,
            NormMode::Percentile  => NormMode::Fixed,
            NormMode::Fixed       => NormMode::Global,
        }
    }
}

/// Scan all frames to find the global finite min/max (NaN and ±inf are ignored).
pub fn global_range(frames: &[Vec<f32>]) -> (f32, f32) {
    let mut range = (f32::INFINITY, f32::NEG_INFINITY);
    for frame in frames {
        range = extend_range(range, frame);
    }
    range
}

/// Fold one frame's finite min/max into a running range. Lets the streaming path
/// update the global range in O(new data) per poll instead of rescanning every
/// buffered frame on every redraw (which is O(N²) over a long animation).
pub fn extend_range(cur: (f32, f32), frame: &[f32]) -> (f32, f32) {
    let (mut mn, mut mx) = cur;
    for &v in frame {
        if v.is_finite() {
            if v < mn { mn = v; }
            if v > mx { mx = v; }
        }
    }
    (mn, mx)
}

/// Compute the vmin/vmax pair for a single frame given the active mode.
/// Non-finite values (NaN, ±inf) are excluded from all range calculations;
/// the shader clamps them to colormap min/max via the normal clamp(t, 0, 1).
pub fn frame_range(
    data:   &[f32],
    mode:   NormMode,
    global: (f32, f32),
    fixed:  (f32, f32),
) -> (f32, f32) {
    let (mn, mx) = match mode {
        NormMode::Global     => global,
        NormMode::Fixed      => fixed,
        NormMode::PerFrame   => {
            let mut mn = f32::INFINITY;
            let mut mx = f32::NEG_INFINITY;
            for &v in data {
                if v.is_finite() {
                    if v < mn { mn = v; }
                    if v > mx { mx = v; }
                }
            }
            (mn, mx)
        }
        NormMode::Percentile => {
            let mut sorted: Vec<f32> = data.iter().cloned().filter(|v| v.is_finite()).collect();
            sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
            let n  = sorted.len();
            if n == 0 { return (0.0, 1.0); }
            let lo = sorted[((0.02 * n as f32) as usize).min(n - 1)];
            let hi = sorted[((0.98 * n as f32) as usize).min(n - 1)];
            (lo, hi)
        }
    };
    // Guard against degenerate range so the shader doesn't divide by zero.
    if (mx - mn).abs() < 1e-9 { (mn, mn + 1.0) } else { (mn, mx) }
}
