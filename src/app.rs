use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{KeyEvent, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{Key, NamedKey},
    window::{Window, WindowAttributes, WindowId},
};

use crate::colormap::Colormap;
use crate::renderer::GpuState;

// ─────────────────────────────────────────────────────────────────────────────
// Test animation: Gaussian blob orbiting the centre, 60 frames @ 30 fps.
// ─────────────────────────────────────────────────────────────────────────────

const W: u32 = 256;
const H: u32 = 256;
const N_FRAMES: u32 = 60;
const FPS: f64 = 30.0;

fn orbit_frames() -> Vec<Vec<f32>> {
    (0..N_FRAMES).map(|i| {
        let angle = 2.0 * std::f32::consts::PI * i as f32 / N_FRAMES as f32;
        let cx = 0.5 + 0.28 * angle.cos();
        let cy = 0.5 + 0.28 * angle.sin();
        (0..H).flat_map(|row| {
            (0..W).map(move |col| {
                let dx = col as f32 / W as f32 - cx;
                let dy = row as f32 / H as f32 - cy;
                (-50.0 * (dx * dx + dy * dy)).exp()
            })
        }).collect()
    }).collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// App
// ─────────────────────────────────────────────────────────────────────────────

pub struct App {
    window:          Option<Arc<Window>>,
    gpu:             Option<GpuState>,
    frames:          Vec<Vec<f32>>,
    frame_idx:       usize,
    fps:             f64,
    // Next absolute time at which to advance to the next animation frame.
    // Nil until the first RedrawRequested fires.
    next_anim_frame: Option<Instant>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            window:          None,
            gpu:             None,
            frames:          Vec::new(),
            frame_idx:       0,
            fps:             FPS,
            next_anim_frame: None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title("wgpu_animator — M4")
            .with_inner_size(LogicalSize::new(900u32, 800u32));

        let window = Arc::new(
            event_loop.create_window(attrs).expect("failed to create window"),
        );

        let mut gpu = GpuState::new(Arc::clone(&window)).expect("failed to init wgpu");

        self.frames = orbit_frames();
        gpu.init_data(W, H, &self.frames[0], 0.0, 1.0, Colormap::Heat);

        self.window          = Some(window);
        self.gpu             = Some(gpu);
        self.frame_idx       = 0;
        self.next_anim_frame = None;

        self.window.as_ref().unwrap().request_redraw();
    }

    // Request a redraw every time the queue drains.  PresentMode::Fifo in
    // render() blocks at vsync, so this loop naturally runs at the display
    // refresh rate — no software WaitUntil timer needed or wanted.
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    fn window_event(
        &mut self,
        event_loop:  &ActiveEventLoop,
        _window_id:  WindowId,
        event:       WindowEvent,
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
                // ── Animation frame advance ───────────────────────────────
                // Rendered at vsync rate (e.g. 60 Hz); animation advances at
                // self.fps (e.g. 30 fps).  The same animation frame is uploaded
                // for multiple display frames — no timer, no WaitUntil jitter.
                if !self.frames.is_empty() {
                    let now = Instant::now();
                    let frame_duration = Duration::from_secs_f64(1.0 / self.fps);
                    let due = self.next_anim_frame.get_or_insert(now + frame_duration);
                    if now >= *due {
                        self.frame_idx = (self.frame_idx + 1) % self.frames.len();
                        *due += frame_duration;
                        // If fallen more than one frame behind, snap forward.
                        if *due < now { *due = now + frame_duration; }
                    }
                }

                // ── Upload + render ───────────────────────────────────────
                if let Some(gpu) = &mut self.gpu {
                    if !self.frames.is_empty() {
                        gpu.upload_frame(&self.frames[self.frame_idx], 0.0, 1.0);
                    }
                    match gpu.render() {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            if let Some(w) = &self.window { gpu.resize(w.inner_size()); }
                        }
                        Err(e) => log::error!("render error: {e}"),
                    }
                }
            }

            _ => {}
        }
    }
}
