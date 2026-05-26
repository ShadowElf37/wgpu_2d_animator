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

## Upcoming

| # | Goal |
|---|---|
| M3 | Colormap LUT texture + all built-in maps |
| M4 | Animation playback at target FPS |
| M5 | Normalization modes (global, per-frame, percentile) |
| M6 | Interpolation switch (nearest → linear → bicubic) |
| M7 | egui axis labels + colorbar ticks |
| M8 | Zoom / pan |
| M9 | MXFR stdin reader + CLI |
| M10 | PyO3 Python extension (optional) |
