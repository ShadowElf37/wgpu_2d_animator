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
use crate::interp::InterpMode;
use crate::norm::{self, NormMode};
use crate::renderer::GpuState;
use crate::ui;

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
    next_anim_frame: Option<Instant>,
    norm_mode:       NormMode,
    global_range:    (f32, f32),
    fixed_range:     (f32, f32),
    interp_mode:     InterpMode,
    colormap:        Colormap,
    egui_ctx:        egui::Context,
    egui_winit:      Option<egui_winit::State>,
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
            norm_mode:       NormMode::default(),
            global_range:    (0.0, 1.0),
            fixed_range:     (0.0, 1.0),
            interp_mode:     InterpMode::default(),
            colormap:        Colormap::default(),
            egui_ctx:        egui::Context::default(),
            egui_winit:      None,
        }
    }
}

impl App {
    fn update_title(&self) {
        if let Some(w) = &self.window {
            w.set_title(&format!(
                "wgpu_animator — norm: {} | interp: {}",
                self.norm_mode.label(),
                self.interp_mode.label(),
            ));
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title("wgpu_animator — M7")
            .with_inner_size(LogicalSize::new(900u32, 800u32));

        let window = Arc::new(
            event_loop.create_window(attrs).expect("failed to create window"),
        );

        // egui-winit state — needs the window for scale factor and display handle.
        let egui_winit = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None, // theme
            None, // max_texture_side (let wgpu backend decide)
        );

        let mut gpu = GpuState::new(Arc::clone(&window)).expect("failed to init wgpu");

        self.frames       = orbit_frames();
        self.global_range = norm::global_range(&self.frames);

        let (vmin, vmax) = norm::frame_range(
            &self.frames[0], self.norm_mode, self.global_range, self.fixed_range,
        );
        gpu.init_data(W, H, &self.frames[0], vmin, vmax, self.colormap, self.interp_mode.as_u32());

        self.window          = Some(window);
        self.gpu             = Some(gpu);
        self.egui_winit      = Some(egui_winit);
        self.frame_idx       = 0;
        self.next_anim_frame = None;

        self.update_title();
        self.window.as_ref().unwrap().request_redraw();
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    fn window_event(
        &mut self,
        event_loop:  &ActiveEventLoop,
        _window_id:  WindowId,
        event:       WindowEvent,
    ) {
        // Forward every event to egui first (handles mouse/text input for the UI).
        if let (Some(ew), Some(w)) = (&mut self.egui_winit, &self.window) {
            let _ = ew.on_window_event(w, &event);
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::KeyboardInput {
                event: KeyEvent { logical_key: Key::Named(NamedKey::Escape), .. },
                ..
            } => event_loop.exit(),

            // N — cycle normalization mode; I — cycle interpolation mode
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    logical_key: Key::Character(ref ch),
                    state: winit::event::ElementState::Pressed,
                    ..
                },
                ..
            } if matches!(ch.as_str(), "n" | "N" | "i" | "I") => {
                match ch.as_str() {
                    "n" | "N" => {
                        self.norm_mode = self.norm_mode.next();
                        log::info!("norm mode: {}", self.norm_mode.label());
                    }
                    _ => {
                        self.interp_mode = self.interp_mode.next();
                        log::info!("interp mode: {}", self.interp_mode.label());
                    }
                }
                self.update_title();
            }

            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu { gpu.resize(size); }
            }

            WindowEvent::RedrawRequested => {
                // ── Animation frame advance ───────────────────────────────
                if !self.frames.is_empty() {
                    let now = Instant::now();
                    let frame_duration = Duration::from_secs_f64(1.0 / self.fps);
                    let due = self.next_anim_frame.get_or_insert(now + frame_duration);
                    if now >= *due {
                        self.frame_idx = (self.frame_idx + 1) % self.frames.len();
                        *due += frame_duration;
                        if *due < now { *due = now + frame_duration; }
                    }
                }

                // ── Compute normalization ─────────────────────────────────
                let (vmin, vmax) = if !self.frames.is_empty() {
                    norm::frame_range(
                        &self.frames[self.frame_idx],
                        self.norm_mode, self.global_range, self.fixed_range,
                    )
                } else { (0.0, 1.0) };
                let interp   = self.interp_mode.as_u32();
                let colormap = self.colormap;

                // ── Upload frame ──────────────────────────────────────────
                if let Some(gpu) = &mut self.gpu {
                    if !self.frames.is_empty() {
                        gpu.upload_frame(&self.frames[self.frame_idx], vmin, vmax, interp);
                    }
                }

                // ── Build egui UI ─────────────────────────────────────────
                let window = self.window.as_ref().unwrap();
                let raw_input = self.egui_winit.as_mut().unwrap().take_egui_input(window);
                let full_output = self.egui_ctx.run(raw_input, |ctx| {
                    ui::build(ctx, vmin, vmax, colormap);
                });
                self.egui_winit.as_mut().unwrap()
                    .handle_platform_output(window, full_output.platform_output);

                let paint_jobs = self.egui_ctx.tessellate(
                    full_output.shapes, full_output.pixels_per_point,
                );
                let size = window.inner_size();
                let screen_desc = egui_wgpu::ScreenDescriptor {
                    size_in_pixels:   [size.width, size.height],
                    pixels_per_point: full_output.pixels_per_point,
                };

                // ── Render ────────────────────────────────────────────────
                if let Some(gpu) = &mut self.gpu {
                    match gpu.render(&paint_jobs, &full_output.textures_delta, screen_desc) {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            gpu.resize(window.inner_size());
                        }
                        Err(e) => log::error!("render error: {e}"),
                    }
                }
            }

            _ => {}
        }
    }
}
