mod app;
mod colormap;
mod interp;
mod norm;
mod renderer;
mod stdin_reader;
mod ui;

use clap::Parser;
use winit::event_loop::EventLoop;

use app::AppConfig;
use colormap::Colormap;
use interp::InterpMode;
use norm::NormMode;

#[derive(Parser)]
#[command(about = "Real-time 2D field animator", long_about = None)]
struct Cli {
    /// Read MXFR binary frames from stdin (pipe from solver or script)
    #[arg(long)]
    stdin: bool,

    /// Playback FPS (animation loop speed)
    #[arg(long, default_value_t = 30.0)]
    fps: f64,

    /// Colormap: heat, inferno, viridis, rdbu, grayscale, galaxy
    #[arg(long, default_value = "heat")]
    colormap: String,

    /// Normalization: global, per-frame, percentile, fixed
    #[arg(long = "norm", default_value = "global")]
    norm: String,

    /// Interpolation: nearest, linear, bicubic
    #[arg(long = "interp", default_value = "nearest")]
    interp: String,

    /// Title text shown in the top-left corner of the animation window
    #[arg(long)]
    title: Option<String>,

    /// Bare mode: hide colorbar and axis-tick overlays (used for image display)
    #[arg(long)]
    bare: bool,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("wgpu=warn,wgpu_animator=info"),
    )
    .init();

    let cli = Cli::parse();

    let stdin_rx = if cli.stdin {
        log::info!("stdin mode: expecting MXFR frames");
        Some(stdin_reader::spawn_reader())
    } else {
        None
    };

    let colormap = match cli.colormap.to_ascii_lowercase().as_str() {
        "inferno"   => Colormap::Inferno,
        "viridis"   => Colormap::Viridis,
        "rdbu"      => Colormap::RdBu,
        "grayscale" => Colormap::Grayscale,
        "galaxy"    => Colormap::Galaxy,
        _           => Colormap::Heat,
    };

    let norm_mode = match cli.norm.to_ascii_lowercase().as_str() {
        "per-frame" | "perframe" | "frame" => NormMode::PerFrame,
        "percentile" | "pct"               => NormMode::Percentile,
        "fixed"                            => NormMode::Fixed,
        _                                  => NormMode::Global,
    };

    let interp_mode = match cli.interp.to_ascii_lowercase().as_str() {
        "linear" | "bilinear"  => InterpMode::Linear,
        "bicubic" | "cubic"    => InterpMode::Bicubic,
        _                      => InterpMode::Nearest,
    };

    let config = AppConfig {
        stdin_rx,
        fps: cli.fps,
        colormap,
        norm_mode,
        interp_mode,
        title: cli.title,
        bare: cli.bare,
    };

    let event_loop = EventLoop::new()?;
    let mut app = app::App::new(config);
    event_loop.run_app(&mut app)?;
    Ok(())
}
