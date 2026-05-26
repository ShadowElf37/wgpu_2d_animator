use std::sync::Arc;
use anyhow::{Context, Result};
use winit::window::Window;

// ─────────────────────────────────────────────────────────────────────────────
// Uniforms — matches the WGSL struct in data.wgsl exactly (16 bytes)
// ─────────────────────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    vmin: f32,
    vmax: f32,
    _pad: [f32; 2],
}

// ─────────────────────────────────────────────────────────────────────────────
// DataPipeline
//
// Owns the render pipeline, the R32Float data texture, and the uniform buffer
// that carries vmin/vmax to the shader.  Created once; reused every frame.
// ─────────────────────────────────────────────────────────────────────────────

pub struct DataPipeline {
    pipeline:    wgpu::RenderPipeline,
    bind_group:  wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    texture:     wgpu::Texture,
    pub width:   u32,
    pub height:  u32,
}

impl DataPipeline {
    pub fn new(
        device:         &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        width:          u32,
        height:         u32,
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
        // binding 1: R32Float texture (non-filterable → textureLoad in shader)
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
                buffers:             &[],   // vertices generated from vertex_index in shader
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
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        // ── Texture ───────────────────────────────────────────────────────
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label:           Some("data texture"),
            size:            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count:    1,
            dimension:       wgpu::TextureDimension::D2,
            format:          wgpu::TextureFormat::R32Float,
            usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats:    &[],
        });
        let tex_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

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
                    resource: wgpu::BindingResource::TextureView(&tex_view),
                },
            ],
        });

        Self { pipeline, bind_group, uniform_buf, texture, width, height }
    }

    /// Upload a new frame and update the normalization range.
    /// `data` must be `width * height` f32 values, row-major.
    pub fn upload(&self, queue: &wgpu::Queue, data: &[f32], vmin: f32, vmax: f32) {
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
                bytes_per_row:  Some(self.width * 4),   // 4 bytes per f32
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
        queue.write_buffer(
            &self.uniform_buf,
            0,
            bytemuck::bytes_of(&Uniforms { vmin, vmax, _pad: [0.0; 2] }),
        );
    }

    /// Record the draw call into an active render pass.
    pub fn draw<'rp>(&'rp self, pass: &mut wgpu::RenderPass<'rp>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..4, 0..1);   // 4 vertices (TriangleStrip), 1 instance
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
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        Ok(Self { _window: window, surface, device, queue, config, data_pipeline: None })
    }

    /// Create the data pipeline for a fixed grid size and upload the first frame.
    pub fn init_data(&mut self, width: u32, height: u32, data: &[f32], vmin: f32, vmax: f32) {
        let dp = DataPipeline::new(&self.device, self.config.format, width, height);
        dp.upload(&self.queue, data, vmin, vmax);
        self.data_pipeline = Some(dp);
    }

    /// Upload a new frame to an already-initialised pipeline.
    pub fn upload_frame(&self, data: &[f32], vmin: f32, vmax: f32) {
        if let Some(dp) = &self.data_pipeline {
            dp.upload(&self.queue, data, vmin, vmax);
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 { return; }
        self.config.width  = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view   = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut enc = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("frame") },
        );

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

        self.queue.submit(std::iter::once(enc.finish()));
        output.present();
        Ok(())
    }
}
