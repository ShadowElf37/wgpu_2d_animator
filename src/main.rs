mod app;
mod renderer;

use winit::event_loop::EventLoop;

fn main() -> anyhow::Result<()> {
    // Default: show wgpu warnings and our own info logs.
    // Override with RUST_LOG env var for more detail (e.g. RUST_LOG=wgpu_core=debug).
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("wgpu=warn,wgpu_animator=info")
    ).init();
    let event_loop = EventLoop::new()?;
    let mut app = app::App::default();
    event_loop.run_app(&mut app)?;
    Ok(())
}
