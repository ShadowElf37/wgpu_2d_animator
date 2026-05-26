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

/// Scan all frames to find the global min/max.
pub fn global_range(frames: &[Vec<f32>]) -> (f32, f32) {
    let mut mn = f32::INFINITY;
    let mut mx = f32::NEG_INFINITY;
    for frame in frames {
        for &v in frame {
            if v < mn { mn = v; }
            if v > mx { mx = v; }
        }
    }
    (mn, mx)
}

/// Compute the vmin/vmax pair for a single frame given the active mode.
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
            let mn = data.iter().cloned().fold(f32::INFINITY,     f32::min);
            let mx = data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            (mn, mx)
        }
        NormMode::Percentile => {
            let mut sorted: Vec<f32> = data.to_vec();
            sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let n  = sorted.len();
            let lo = sorted[((0.02 * n as f32) as usize).min(n - 1)];
            let hi = sorted[((0.98 * n as f32) as usize).min(n - 1)];
            (lo, hi)
        }
    };
    // Guard against degenerate range so the shader doesn't divide by zero.
    if (mx - mn).abs() < 1e-9 { (mn, mn + 1.0) } else { (mn, mx) }
}
