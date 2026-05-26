use std::sync::Arc;
use anyhow::{Context, Result};
use winit::window::Window;

// ─────────────────────────────────────────────────────────────────────────────
// GpuState
//
// Owns everything wgpu needs for a single window: surface, device, queue, and
// the swapchain config.  For M1 the only rendering operation is clearing the
// framebuffer to a solid background colour.
// ─────────────────────────────────────────────────────────────────────────────

pub struct GpuState {
    // Keep the Arc alive so the surface's internal window handle stays valid.
    _window:  Arc<Window>,
    surface:  wgpu::Surface<'static>,
    device:   wgpu::Device,
    queue:    wgpu::Queue,
    config:   wgpu::SurfaceConfiguration,
}

impl GpuState {
    /// Initialise wgpu for the given window.  Blocks the caller via pollster.
    pub fn new(window: Arc<Window>) -> Result<Self> {
        pollster::block_on(Self::new_async(window))
    }

    async fn new_async(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();

        // ── Instance ─────────────────────────────────────────────────────────
        // Prefer native backends (Vulkan/Metal/DX12); fall back to GL.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // ── Surface ──────────────────────────────────────────────────────────
        // Arc<Window> implements Into<SurfaceTarget<'static>>, so the surface
        // lifetime is 'static as long as the Arc is kept alive.
        let surface = instance
            .create_surface(Arc::clone(&window))
            .context("failed to create wgpu surface")?;

        // ── Adapter ──────────────────────────────────────────────────────────
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference:       wgpu::PowerPreference::HighPerformance,
                compatible_surface:     Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("no suitable GPU adapter found")?;

        log::info!("GPU: {}", adapter.get_info().name);

        // ── Device + Queue ───────────────────────────────────────────────────
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .context("failed to acquire wgpu device")?;

        // ── Surface configuration (swapchain) ─────────────────────────────
        let caps   = surface.get_capabilities(&adapter);
        let format = caps.formats[0];   // first format is always a sensible default

        let config = wgpu::SurfaceConfiguration {
            usage:                        wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width:                        size.width.max(1),
            height:                       size.height.max(1),
            present_mode:                 wgpu::PresentMode::Fifo, // vsync
            alpha_mode:                   caps.alpha_modes[0],
            view_formats:                 vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        Ok(Self { _window: window, surface, device, queue, config })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // resize / render
    // ─────────────────────────────────────────────────────────────────────────

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.config.width  = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Clear the framebuffer to the background colour and present it.
    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view   = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut enc = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("frame") },
        );

        // A render pass that does nothing except clear the screen.
        // The dark blue-grey colour distinguishes "renderer alive" from a
        // blank window, so it serves as an explicit M1 success indicator.
        {
            let _pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
            });
        } // render pass dropped → recorded into the command buffer

        self.queue.submit(std::iter::once(enc.finish()));
        output.present();
        Ok(())
    }
}
