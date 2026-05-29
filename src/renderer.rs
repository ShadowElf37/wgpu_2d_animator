use std::sync::Arc;
use anyhow::{Context, Result};
use winit::window::Window;

use crate::colormap::Colormap;

// ─────────────────────────────────────────────────────────────────────────────
// Uniforms — matches the WGSL struct in data.wgsl exactly (16 bytes)
// ─────────────────────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    vmin:        f32,
    vmax:        f32,
    interp_mode: u32,
    channels:    u32,
    pan:         [f32; 2],
    zoom:        f32,
    _pad2:       f32,
}

// ─────────────────────────────────────────────────────────────────────────────
// DataPipeline
//
// Owns the render pipeline, the R32Float data texture, a 1D Rgba8Unorm
// colormap LUT texture with linear sampler, and the vmin/vmax uniform buffer.
// Created once per colormap change; reused every frame.
// ─────────────────────────────────────────────────────────────────────────────

pub struct DataPipeline {
    pipeline:      wgpu::RenderPipeline,
    bind_group:    wgpu::BindGroup,
    uniform_buf:   wgpu::Buffer,
    texture:       wgpu::Texture,
    cmap_texture:  wgpu::Texture,
    _cmap_sampler: wgpu::Sampler,
    pub width:     u32,
    pub height:    u32,
    pub channels:  u32,
}

impl DataPipeline {
    pub fn new(
        device:         &wgpu::Device,
        queue:          &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        width:          u32,
        height:         u32,
        colormap:       Colormap,
        channels:       u32,
    ) -> Self {
        // ── Shader ───────────────────────────────────────────────────────
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("data shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("shaders/data.wgsl").into()
            ),
        });

        // ── Bind group layout ─────────────────────────────────────────────
        // binding 0: uniform buffer (vmin, vmax)
        // binding 1: R32Float data texture (non-filterable → textureLoad)
        // binding 2: Rgba8Unorm colormap LUT 1D texture (filterable)
        // binding 3: linear sampler for the LUT
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("data bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding:    0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty:                 wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size:   wgpu::BufferSize::new(
                            std::mem::size_of::<Uniforms>() as u64
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type:    wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled:   false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type:    wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D1,
                        multisampled:   false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding:    3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // ── Pipeline ──────────────────────────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label:                Some("data pipeline layout"),
                bind_group_layouts:   &[&bgl],
                push_constant_ranges: &[],
            },
        );

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("data pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module:              &shader,
                entry_point:         "vs_main",
                buffers:             &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:              &shader,
                entry_point:         "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format:     surface_format,
                    blend:      None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        // ── Data texture — R32Float for scalar, Rgba32Float for RGB ──────
        let tex_format = if channels == 3 {
            wgpu::TextureFormat::Rgba32Float
        } else {
            wgpu::TextureFormat::R32Float
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label:           Some("data texture"),
            size:            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count:    1,
            dimension:       wgpu::TextureDimension::D2,
            format:          tex_format,
            usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats:    &[],
        });
        let data_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // ── Colormap LUT texture (Rgba8Unorm, 1D, filterable) ─────────────
        let cmap_texture = device.create_texture(&wgpu::TextureDescriptor {
            label:           Some("colormap LUT"),
            size:            wgpu::Extent3d { width: 256, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count:    1,
            dimension:       wgpu::TextureDimension::D1,
            format:          wgpu::TextureFormat::Rgba8Unorm,
            usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats:    &[],
        });
        let cmap_view = cmap_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D1),
            ..Default::default()
        });

        // Upload initial LUT data.
        Self::write_lut(queue, &cmap_texture, colormap);

        // ── Colormap sampler (linear, clamp-to-edge) ──────────────────────
        let cmap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label:            Some("cmap sampler"),
            address_mode_u:   wgpu::AddressMode::ClampToEdge,
            mag_filter:       wgpu::FilterMode::Linear,
            min_filter:       wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── Uniform buffer ────────────────────────────────────────────────
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("uniforms"),
            size:               std::mem::size_of::<Uniforms>() as u64,
            usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Bind group ────────────────────────────────────────────────────
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("data bg"),
            layout:  &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding:  0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding:  1,
                    resource: wgpu::BindingResource::TextureView(&data_view),
                },
                wgpu::BindGroupEntry {
                    binding:  2,
                    resource: wgpu::BindingResource::TextureView(&cmap_view),
                },
                wgpu::BindGroupEntry {
                    binding:  3,
                    resource: wgpu::BindingResource::Sampler(&cmap_sampler),
                },
            ],
        });

        Self {
            pipeline,
            bind_group,
            uniform_buf,
            texture,
            cmap_texture,
            _cmap_sampler: cmap_sampler,
            width,
            height,
            channels,
        }
    }

    /// Swap to a different colormap without recreating the pipeline.
    pub fn set_colormap(&self, queue: &wgpu::Queue, colormap: Colormap) {
        Self::write_lut(queue, &self.cmap_texture, colormap);
    }

    fn write_lut(queue: &wgpu::Queue, tex: &wgpu::Texture, colormap: Colormap) {
        let lut = colormap.lut_rgba8();
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture:   tex,
                mip_level: 0,
                origin:    wgpu::Origin3d::ZERO,
                aspect:    wgpu::TextureAspect::All,
            },
            &lut,
            wgpu::ImageDataLayout {
                offset:         0,
                bytes_per_row:  Some(256 * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d { width: 256, height: 1, depth_or_array_layers: 1 },
        );
    }

    /// Upload a new frame, normalization range, interpolation mode, and zoom/pan.
    /// For scalar (channels==1): `data` is `width * height` f32 values.
    /// For RGB (channels==3): `data` is `width * height * 3` f32 values (R,G,B interleaved, [0,1]).
    pub fn upload(
        &self,
        queue:       &wgpu::Queue,
        data:        &[f32],
        vmin:        f32,
        vmax:        f32,
        interp_mode: u32,
        pan:         [f32; 2],
        zoom:        f32,
    ) {
        if self.channels == 3 {
            // Pack RGB → RGBA for Rgba32Float texture.
            let rgba: Vec<f32> = data.chunks_exact(3)
                .flat_map(|c| [c[0], c[1], c[2], 1.0_f32])
                .collect();
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture:   &self.texture,
                    mip_level: 0,
                    origin:    wgpu::Origin3d::ZERO,
                    aspect:    wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&rgba),
                wgpu::ImageDataLayout {
                    offset:         0,
                    bytes_per_row:  Some(self.width * 16), // 4 channels * 4 bytes
                    rows_per_image: Some(self.height),
                },
                wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
            );
        } else {
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture:   &self.texture,
                    mip_level: 0,
                    origin:    wgpu::Origin3d::ZERO,
                    aspect:    wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(data),
                wgpu::ImageDataLayout {
                    offset:         0,
                    bytes_per_row:  Some(self.width * 4),
                    rows_per_image: Some(self.height),
                },
                wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
            );
        }
        queue.write_buffer(
            &self.uniform_buf,
            0,
            bytemuck::bytes_of(&Uniforms {
                vmin, vmax, interp_mode, channels: self.channels, pan, zoom, _pad2: 0.0,
            }),
        );
    }

    /// Record the draw call into an active render pass.
    pub fn draw<'rp>(&'rp self, pass: &mut wgpu::RenderPass<'rp>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GpuState
// ─────────────────────────────────────────────────────────────────────────────

pub struct GpuState {
    _window:       Arc<Window>,
    surface:       wgpu::Surface<'static>,
    device:        wgpu::Device,
    queue:         wgpu::Queue,
    config:        wgpu::SurfaceConfiguration,
    data_pipeline: Option<DataPipeline>,
    egui_renderer: egui_wgpu::Renderer,
}

impl GpuState {
    pub fn new(window: Arc<Window>) -> Result<Self> {
        pollster::block_on(Self::new_async(window))
    }

    async fn new_async(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(Arc::clone(&window))
            .context("failed to create wgpu surface")?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference:       wgpu::PowerPreference::HighPerformance,
                compatible_surface:     Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("no suitable GPU adapter")?;

        log::info!("GPU: {}", adapter.get_info().name);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .context("failed to acquire wgpu device")?;

        let caps   = surface.get_capabilities(&adapter);
        let format = caps.formats[0];

        let config = wgpu::SurfaceConfiguration {
            usage:                        wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width:                        size.width.max(1),
            height:                       size.height.max(1),
            present_mode:                 wgpu::PresentMode::Fifo,
            alpha_mode:                   caps.alpha_modes[0],
            view_formats:                 vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        let egui_renderer = egui_wgpu::Renderer::new(&device, format, None, 1, false);

        Ok(Self { _window: window, surface, device, queue, config, data_pipeline: None, egui_renderer })
    }

    /// Create the data pipeline for a fixed grid size and upload the first frame.
    pub fn init_data(
        &mut self,
        width:       u32,
        height:      u32,
        data:        &[f32],
        vmin:        f32,
        vmax:        f32,
        colormap:    Colormap,
        interp_mode: u32,
        channels:    u32,
    ) {
        let dp = DataPipeline::new(
            &self.device, &self.queue, self.config.format, width, height, colormap, channels,
        );
        dp.upload(&self.queue, data, vmin, vmax, interp_mode, [0.0, 0.0], 1.0);
        self.data_pipeline = Some(dp);
    }

    /// Upload a new frame to an already-initialised pipeline.
    pub fn upload_frame(&self, data: &[f32], vmin: f32, vmax: f32, interp_mode: u32, pan: [f32; 2], zoom: f32) {
        if let Some(dp) = &self.data_pipeline {
            dp.upload(&self.queue, data, vmin, vmax, interp_mode, pan, zoom);
        }
    }

    /// Swap the colormap without recreating the pipeline.
    pub fn set_colormap(&self, colormap: Colormap) {
        if let Some(dp) = &self.data_pipeline {
            dp.set_colormap(&self.queue, colormap);
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 { return; }
        self.config.width  = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(
        &mut self,
        paint_jobs:     &[egui::ClippedPrimitive],
        textures_delta: &egui::TexturesDelta,
        screen_desc:    egui_wgpu::ScreenDescriptor,
    ) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        // Explicitly set the view format to match the surface config.
        // On macOS/Metal the swapchain texture's internal format can differ from
        // the configured sRGB format; using the default descriptor would pick the
        // internal format and silently mismatch the pipeline's colour target.
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.config.format),
            ..Default::default()
        });

        // Upload egui texture changes and vertex/index buffers.
        for (id, delta) in &textures_delta.set {
            self.egui_renderer.update_texture(&self.device, &self.queue, *id, delta);
        }

        let mut enc = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("frame") },
        );

        self.egui_renderer.update_buffers(
            &self.device, &self.queue, &mut enc, paint_jobs, &screen_desc,
        );

        // ── Data pass (clear → draw data) ────────────────────────────────────
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("data pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.05, b: 0.12, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
            });
            if let Some(dp) = &self.data_pipeline {
                dp.draw(&mut pass);
            }
        }

        // ── egui pass (load → composite UI on top) ────────────────────────────
        // egui_wgpu::Renderer::render requires RenderPass<'static>; wgpu 22
        // provides forget_lifetime() for exactly this use case (egui owns all
        // resources referenced by the pass, so 'static is safe).
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view:           &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
            }).forget_lifetime();
            self.egui_renderer.render(&mut pass, paint_jobs, &screen_desc);
        }

        // Free egui textures that are no longer needed.
        for id in &textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        self.queue.submit(std::iter::once(enc.finish()));
        output.present();
        Ok(())
    }
}
