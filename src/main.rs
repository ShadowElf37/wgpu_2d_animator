mod app;
mod renderer;

use winit::event_loop::EventLoop;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let event_loop = EventLoop::new()?;
    let mut app = app::App::default();
    event_loop.run_app(&mut app)?;
    Ok(())
}
