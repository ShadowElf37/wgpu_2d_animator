# wgpu_animator — Progress

Milestone tracker. Each entry records what was implemented and any non-obvious decisions made during that milestone.

---

## M1 — Window + clear pass ✅

**Files:** `src/main.rs`, `src/app.rs`, `src/renderer.rs`

- `winit 0.30` `ApplicationHandler` trait used for the event loop.  Window and `GpuState` are created lazily in `resumed()` because a valid window handle does not exist before that point.
- `wgpu 22` surface configured with `PresentMode::Fifo` (vsync).  `Arc<Window>` passed to `create_surface` so the surface gets `'static` lifetime while `GpuState` keeps its own `Arc` clone to guarantee the window stays alive.
- Render loop driven by calling `request_redraw()` at the end of each `RedrawRequested` event — continuous render, no sleep.  Will be replaced by `WaitUntil` at M3.
- Clear colour `(0.05, 0.05, 0.12)` (dark blue-grey) chosen to be visually distinct from a blank OS window.

---

## M2 — R32Float texture + heat colormap shader ✅

**Files:** `src/renderer.rs` (added `DataPipeline`), `src/shaders/data.wgsl`, `src/app.rs` (test data)

- `DataPipeline` holds the render pipeline, R32Float texture, uniform buffer (vmin/vmax), and bind group.  Lives inside `GpuState` as `Option<DataPipeline>`.
- **No vertex buffer:** the vertex shader generates a full-screen quad (triangle strip, 4 vertices) from `@builtin(vertex_index)`.  Saves a buffer allocation and a pipeline vertex layout declaration.
- **`textureLoad` not `textureSample`:** `R32Float` is not filterable on Metal/Vulkan without enabling `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`.  `textureLoad` with an integer coordinate gives nearest-neighbour sampling portably.  Bilinear/bicubic are deferred to M6.
- **Heat colormap inline:** black→red→white expressed as three piecewise-linear ramps directly in the fragment shader.  A 1D LUT texture will replace this at M3 when all colormaps are added.
- Uniform struct padded to 16 bytes (`vmin`, `vmax`, `_pad: vec2<f32>`) to satisfy wgpu uniform buffer alignment on all backends.
- Test data: 256×256 Gaussian blob centred at (0.5, 0.5).  Provides a smooth gradient that exercises the full [0, 1] colormap range.

**Bug fixed post-commit:**
- `RedrawRequested` does not fire automatically on macOS before an explicit call.  Fixed by calling `request_redraw()` at the end of `resumed()` and implementing `about_to_wait()` as the continuous driver.
- Texture view created with `format: Some(self.config.format)` instead of `Default::default()`.  On macOS/Metal the swapchain texture's internal format can differ from the configured sRGB surface format; the default descriptor picks the internal format, silently mismatching the pipeline's colour target and causing the draw to be discarded.
- Topology changed from `TriangleStrip` (4 vertices) to `TriangleList` (6 vertices) — less ambiguous, no degenerate-strip edge cases on any backend.

---

## M3 — Colormap LUT texture + built-in maps ✅

**Files:** `src/colormap.rs` (new), `src/renderer.rs`, `src/shaders/data.wgsl`

- Replaced inline `heat()` fragment shader function with a 256-entry **Rgba8Unorm 1D LUT texture** sampled with `textureSample`.  Using a filterable format gives smooth linear interpolation between LUT entries at no extra cost.
- Five built-in colormaps defined as piecewise-linear RGB stops in `colormap.rs`:
  - **Heat** — black → red → white (previous behaviour, still default)
  - **Inferno** — black → purple → orange → yellow (perceptually uniform)
  - **Viridis** — deep purple → teal → yellow (perceptually uniform)
  - **RdBu** — red → neutral grey → blue (diverging; useful for signed fields like Hz)
  - **Grayscale** — black → white
- `Colormap::lut_rgba8()` linearly interpolates between stops and returns 1024 raw bytes ready for `queue.write_texture`.
- `DataPipeline::set_colormap()` re-uploads only the LUT bytes — no pipeline recreation needed when the colormap is changed at runtime.
- Bind group layout gained two new entries: binding 2 (filterable 1D texture), binding 3 (filtering sampler, `ClampToEdge`, `Linear`).
- `GpuState::init_data` now accepts a `Colormap` argument; `GpuState::set_colormap` exposes hot-swap for future UI use.
- R32Float data texture (non-filterable) still uses `textureLoad` with integer coordinates.

---

## M4 — Animation playback at target FPS ✅

**Files:** `src/app.rs`

- `App` now stores a `Vec<Vec<f32>>` frame sequence and advances `frame_idx` after each `RedrawRequested`.
- Replaced the M2/M3 spin-loop (`request_redraw()` in `about_to_wait` unconditionally) with a **`WaitUntil` schedule**: `about_to_wait` computes `last_frame_time + frame_duration`; if that instant has passed it requests an immediate redraw, otherwise it sets `ControlFlow::WaitUntil(next)` so the OS sleeps the process until the frame is due.  CPU load is now negligible between frames.
- Test animation: 60-frame Gaussian blob orbiting the window centre at radius 0.28, looping at 30 fps.  Exercises the upload-per-frame path and makes timing visually obvious.
- `last_frame_time: Option<Instant>` initialised to `None`; first frame fires immediately.
- `GpuState::upload_frame` called before `render` each `RedrawRequested` so the texture is always current before the draw.

## Upcoming

| # | Goal |
|---|---|
| M5 | Normalization modes (global, per-frame, percentile) |
| M5 | Normalization modes (global, per-frame, percentile) |
| M6 | Interpolation switch (nearest → linear → bicubic) |
| M7 | egui axis labels + colorbar ticks |
| M8 | Zoom / pan |
| M9 | MXFR stdin reader + CLI |
| M10 | PyO3 Python extension (optional) |

