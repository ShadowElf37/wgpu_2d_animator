use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{KeyEvent, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{Key, NamedKey},
    window::{Window, WindowAttributes, WindowId},
};

use crate::renderer::GpuState;

// ─────────────────────────────────────────────────────────────────────────────
// Test data: 256×256 Gaussian blob, values in [0, 1].
// Exercises the full colormap range: dark edges (black), bright centre (white).
// ─────────────────────────────────────────────────────────────────────────────

const W: u32 = 256;
const H: u32 = 256;

fn gaussian_test_frame() -> Vec<f32> {
    (0..H).flat_map(|row| {
        (0..W).map(move |col| {
            let dx = col as f32 / W as f32 - 0.5;
            let dy = row as f32 / H as f32 - 0.5;
            (-30.0 * (dx * dx + dy * dy)).exp()
        })
    }).collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// App
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct App {
    window: Option<Arc<Window>>,
    gpu:    Option<GpuState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title("wgpu_animator — M2")
            .with_inner_size(LogicalSize::new(900u32, 800u32));

        let window = Arc::new(
            event_loop.create_window(attrs).expect("failed to create window"),
        );

        let mut gpu = GpuState::new(Arc::clone(&window)).expect("failed to init wgpu");

        // Upload the Gaussian test frame so there is visible data immediately.
        let frame = gaussian_test_frame();
        gpu.init_data(W, H, &frame, 0.0, 1.0);

        self.window = Some(window);
        self.gpu    = Some(gpu);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event:      WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::KeyboardInput {
                event: KeyEvent { logical_key: Key::Named(NamedKey::Escape), .. },
                ..
            } => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu { gpu.resize(size); }
            }

            WindowEvent::RedrawRequested => {
                if let Some(gpu) = &mut self.gpu {
                    match gpu.render() {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {}
                        Err(e) => log::error!("render error: {e}"),
                    }
                }
                if let Some(w) = &self.window { w.request_redraw(); }
            }

            _ => {}
        }
    }
}
