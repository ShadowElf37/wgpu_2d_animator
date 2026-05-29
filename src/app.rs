use std::sync::Arc;
use std::sync::mpsc;
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
use crate::stdin_reader::MxfrFrame;
use crate::ui;

// ─────────────────────────────────────────────────────────────────────────────
// Test animation (used when no --stdin flag is given)
// ─────────────────────────────────────────────────────────────────────────────

const W: u32 = 256;
const H: u32 = 256;
const N_FRAMES: u32 = 60;

fn orbit_frames() -> Vec<Vec<f32>> {
    (0..N_FRAMES)
        .map(|i| {
            let angle = 2.0 * std::f32::consts::PI * i as f32 / N_FRAMES as f32;
            let cx = 0.5 + 0.28 * angle.cos();
            let cy = 0.5 + 0.28 * angle.sin();
            (0..H)
                .flat_map(|row| {
                    (0..W).map(move |col| {
                        let dx = col as f32 / W as f32 - cx;
                        let dy = row as f32 / H as f32 - cy;
                        (-50.0 * (dx * dx + dy * dy)).exp()
                    })
                })
                .collect()
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// AppConfig
// ─────────────────────────────────────────────────────────────────────────────

pub struct AppConfig {
    pub stdin_rx:    Option<mpsc::Receiver<MxfrFrame>>,
    pub fps:         f64,
    pub colormap:    Colormap,
    pub norm_mode:   NormMode,
    pub interp_mode: InterpMode,
    pub title:       Option<String>,
    pub bare:        bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// App
// ─────────────────────────────────────────────────────────────────────────────

pub struct App {
    window:          Option<Arc<Window>>,
    gpu:             Option<GpuState>,

    // Frame data — common to both modes
    frames:          Vec<Vec<f32>>,
    timestamps:      Vec<f64>,
    frame_idx:       usize,
    fps:             f64,
    next_anim_frame: Option<Instant>,

    // Normalization
    norm_mode:       NormMode,
    global_range:    (f32, f32),
    fixed_range:     (f32, f32),

    // Rendering options
    interp_mode:     InterpMode,
    colormap:        Colormap,
    zoom:            f32,
    pan:             [f32; 2],

    // User-supplied title (shown top-left)
    title:           Option<String>,

    // Playback control
    paused:          bool,

    // Bare mode: no colorbar/axis ticks overlay
    bare:            bool,

    // Stdin streaming
    stdin_rx:        Option<mpsc::Receiver<MxfrFrame>>,
    stream_w:        u32,
    stream_h:        u32,
    stream_channels: u32,
    gpu_initialized: bool,  // true once init_data has been called

    // egui
    egui_ctx:        egui::Context,
    egui_winit:      Option<egui_winit::State>,
}

impl App {
    pub fn new(config: AppConfig) -> Self {
        Self {
            window:          None,
            gpu:             None,
            frames:          Vec::new(),
            timestamps:      Vec::new(),
            frame_idx:       0,
            fps:             config.fps,
            next_anim_frame: None,
            norm_mode:       config.norm_mode,
            global_range:    (0.0, 1.0),
            fixed_range:     (0.0, 1.0),
            interp_mode:     config.interp_mode,
            colormap:        config.colormap,
            zoom:            1.0,
            pan:             [0.0, 0.0],
            title:           config.title,
            paused:          false,
            bare:            config.bare,
            stdin_rx:        config.stdin_rx,
            stream_w:        0,
            stream_h:        0,
            stream_channels: 1,
            gpu_initialized: false,
            egui_ctx:        egui::Context::default(),
            egui_winit:      None,
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn update_title(&self) {
        let Some(w) = &self.window else { return };
        let n = self.frames.len();
        let frame_info = if n > 0 {
            let idx = self.frame_idx + 1;
            if !self.timestamps.is_empty() {
                format!(" | frame {idx}/{n} | t={:.3}", self.timestamps[self.frame_idx.min(self.timestamps.len()-1)])
            } else {
                format!(" | frame {idx}/{n}")
            }
        } else if self.stdin_rx.is_some() {
            " | waiting for frames…".into()
        } else {
            String::new()
        };
        let paused_str = if self.paused { " | PAUSED" } else { "" };
        w.set_title(&format!(
            "wgpu_animator — norm: {} | interp: {} | cmap: {}{}{}",
            self.norm_mode.label(),
            self.interp_mode.label(),
            self.colormap.label(),
            frame_info,
            paused_str,
        ));
    }

    /// Drain all pending stdin frames into the buffer.
    /// Returns true if at least one new frame arrived.
    fn poll_stdin(&mut self) -> bool {
        let mut incoming: Vec<MxfrFrame> = Vec::new();
        let mut disconnected = false;

        {
            let Some(rx) = self.stdin_rx.as_ref() else {
                return false;
            };
            loop {
                match rx.try_recv() {
                    Ok(frame)                             => incoming.push(frame),
                    Err(mpsc::TryRecvError::Empty)        => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        } // borrow of self.stdin_rx ends here

        let got_new = !incoming.is_empty();
        for frame in incoming {
            if self.stream_w == 0 {
                self.stream_w        = frame.width;
                self.stream_h        = frame.height;
                self.stream_channels = frame.channels;
            }
            self.timestamps.push(frame.timestamp);
            self.frames.push(frame.data);
        }

        if disconnected {
            log::info!("stdin stream ended — {} frames buffered", self.frames.len());
            self.stdin_rx = None;
            // Compute global range now that we have all frames.
            if !self.frames.is_empty() {
                self.global_range = norm::global_range(&self.frames);
            }
        } else if got_new && matches!(self.norm_mode, NormMode::Global) {
            // Update incrementally while frames still arrive.
            self.global_range = norm::global_range(&self.frames);
        }

        got_new
    }

    /// Initialize the GPU data pipeline from either test animation or the
    /// first stdin frame.  Must be called after `gpu` is set and we have
    /// at least one frame.
    fn try_init_gpu_data(&mut self) {
        if self.gpu_initialized { return; }
        let gpu = match &mut self.gpu { Some(g) => g, None => return };
        if self.frames.is_empty() { return; }

        let w = if self.stream_w > 0 { self.stream_w } else { W };
        let h = if self.stream_h > 0 { self.stream_h } else { H };

        let (vmin, vmax) = norm::frame_range(
            &self.frames[0], self.norm_mode, self.global_range, self.fixed_range,
        );
        gpu.init_data(w, h, &self.frames[0], vmin, vmax, self.colormap, self.interp_mode.as_u32(), self.stream_channels);
        self.gpu_initialized = true;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ApplicationHandler
// ─────────────────────────────────────────────────────────────────────────────

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title("wgpu_animator")
            .with_inner_size(LogicalSize::new(900u32, 800u32));

        let window = Arc::new(
            event_loop.create_window(attrs).expect("failed to create window"),
        );

        let egui_winit = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        let gpu = GpuState::new(Arc::clone(&window)).expect("failed to init wgpu");

        // Test animation mode: load frames eagerly.
        if self.stdin_rx.is_none() && self.frames.is_empty() {
            self.frames       = orbit_frames();
            self.global_range = norm::global_range(&self.frames);
        }

        self.window          = Some(window);
        self.gpu             = Some(gpu);
        self.egui_winit      = Some(egui_winit);
        self.frame_idx       = 0;
        self.next_anim_frame = None;
        self.gpu_initialized = false;

        // Init GPU data now if we already have frames (test mode); defer if
        // we're waiting for stdin frames.
        if !self.frames.is_empty() {
            self.try_init_gpu_data();
        }

        self.update_title();
        self.window.as_ref().unwrap().request_redraw();
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event:      WindowEvent,
    ) {
        // Forward events to egui first.
        if let (Some(ew), Some(w)) = (&mut self.egui_winit, &self.window) {
            let _ = ew.on_window_event(w, &event);
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::KeyboardInput {
                event: KeyEvent { logical_key: Key::Named(NamedKey::Escape), .. },
                ..
            } => event_loop.exit(),

            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    logical_key: Key::Character(ref ch),
                    state: winit::event::ElementState::Pressed,
                    ..
                },
                ..
            } if matches!(ch.as_str(), "n" | "N" | "i" | "I" | "c" | "C" | "r" | "R") => {
                match ch.as_str() {
                    "n" | "N" => {
                        self.norm_mode = self.norm_mode.next();
                        log::info!("norm mode: {}", self.norm_mode.label());
                    }
                    "i" | "I" => {
                        self.interp_mode = self.interp_mode.next();
                        log::info!("interp mode: {}", self.interp_mode.label());
                    }
                    "c" | "C" => {
                        self.colormap = self.colormap.next();
                        if let Some(gpu) = &self.gpu {
                            gpu.set_colormap(self.colormap);
                        }
                        log::info!("colormap: {}", self.colormap.label());
                    }
                    _ => {
                        self.zoom = 1.0;
                        self.pan  = [0.0, 0.0];
                        log::info!("view reset");
                    }
                }
                self.update_title();
            }

            // Space: toggle pause/play
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    logical_key: Key::Named(NamedKey::Space),
                    state: winit::event::ElementState::Pressed,
                    ..
                },
                ..
            } => {
                self.paused = !self.paused;
                if !self.paused {
                    // Reset the timer so we don't skip frames after unpausing
                    self.next_anim_frame = None;
                }
                log::info!("{}", if self.paused { "paused" } else { "playing" });
                self.update_title();
            }

            // Left arrow: go to previous frame (and pause)
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    logical_key: Key::Named(NamedKey::ArrowLeft),
                    state: winit::event::ElementState::Pressed,
                    ..
                },
                ..
            } => {
                if !self.frames.is_empty() {
                    self.paused    = true;
                    let n          = self.frames.len();
                    self.frame_idx = (self.frame_idx + n - 1) % n;
                    self.update_title();
                }
            }

            // Right arrow: go to next frame (and pause)
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    logical_key: Key::Named(NamedKey::ArrowRight),
                    state: winit::event::ElementState::Pressed,
                    ..
                },
                ..
            } => {
                if !self.frames.is_empty() {
                    self.paused    = true;
                    self.frame_idx = (self.frame_idx + 1) % self.frames.len();
                    self.update_title();
                }
            }

            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu { gpu.resize(size); }
            }

            WindowEvent::RedrawRequested => {
                // ── 1. Pull new stdin frames ──────────────────────────────
                self.poll_stdin();

                // ── 2. Lazy GPU init on first frame ───────────────────────
                self.try_init_gpu_data();

                // ── 3. Animation frame advance ────────────────────────────
                if !self.frames.is_empty() && self.gpu_initialized && !self.paused {
                    let now           = Instant::now();
                    let frame_dur     = Duration::from_secs_f64(1.0 / self.fps);
                    let due           = self.next_anim_frame.get_or_insert(now + frame_dur);
                    if now >= *due {
                        self.frame_idx = (self.frame_idx + 1) % self.frames.len();
                        *due += frame_dur;
                        if *due < now { *due = now + frame_dur; }
                        self.update_title();
                    }
                }

                // ── 4. Normalization ──────────────────────────────────────
                let (vmin, vmax) = if !self.frames.is_empty() && self.gpu_initialized {
                    norm::frame_range(
                        &self.frames[self.frame_idx],
                        self.norm_mode, self.global_range, self.fixed_range,
                    )
                } else {
                    (0.0, 1.0)
                };
                let interp   = self.interp_mode.as_u32();
                let colormap = self.colormap;

                // ── 5. GPU upload ─────────────────────────────────────────
                if let Some(gpu) = &mut self.gpu {
                    if !self.frames.is_empty() && self.gpu_initialized {
                        gpu.upload_frame(
                            &self.frames[self.frame_idx], vmin, vmax, interp,
                            self.pan, self.zoom,
                        );
                    }
                }

                // ── 6. egui UI ────────────────────────────────────────────
                let mut zoom = self.zoom;
                let mut pan  = self.pan;
                let window   = self.window.as_ref().unwrap();
                let raw_input = self.egui_winit.as_mut().unwrap().take_egui_input(window);
                let title = self.title.as_deref();
                let bare  = self.bare;
                let full_output = self.egui_ctx.run(raw_input, |ctx| {
                    ui::build(ctx, vmin, vmax, colormap, &mut zoom, &mut pan, title, bare);
                });
                self.zoom = zoom;
                self.pan  = pan;

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

                // ── 7. Render ─────────────────────────────────────────────
                if let Some(gpu) = &mut self.gpu {
                    match gpu.render(&paint_jobs, &full_output.textures_delta, screen_desc) {
                        Ok(())                                                     => {}
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
