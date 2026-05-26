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
// App
//
// Implements winit's ApplicationHandler trait.  Fields are Option<_> because
// the window and GPU state can't be created until winit fires `resumed()` —
// that's the first point at which a valid window handle exists on all platforms.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct App {
    window: Option<Arc<Window>>,
    gpu:    Option<GpuState>,
}

impl ApplicationHandler for App {
    /// Called once when the event loop is ready to accept a window.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title("Maxwell Animator — M1")
            .with_inner_size(LogicalSize::new(900u32, 800u32));

        let window = Arc::new(
            event_loop.create_window(attrs).expect("failed to create window"),
        );

        let gpu = GpuState::new(Arc::clone(&window)).expect("failed to init wgpu");

        self.window = Some(window);
        self.gpu    = Some(gpu);
    }

    fn window_event(
        &mut self,
        event_loop:  &ActiveEventLoop,
        _window_id:  WindowId,
        event:       WindowEvent,
    ) {
        match event {
            // ── Exit conditions ───────────────────────────────────────────
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    logical_key: Key::Named(NamedKey::Escape),
                    ..
                },
                ..
            } => {
                event_loop.exit();
            }

            // ── Resize: reconfigure the swapchain ─────────────────────────
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size);
                }
            }

            // ── Redraw: clear and present ──────────────────────────────────
            WindowEvent::RedrawRequested => {
                if let Some(gpu) = &mut self.gpu {
                    match gpu.render() {
                        Ok(()) => {}
                        // Surface lost or outdated: the next Resized event will
                        // reconfigure it; nothing to do here.
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {}
                        Err(e) => log::error!("render error: {e}"),
                    }
                }
                // Request another frame immediately (continuous render loop).
                // M3+ will replace this with a timed WaitUntil for target FPS.
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            _ => {}
        }
    }
}
