# wgpu_animator — Producer Protocol

This document describes everything a producer program needs to know to stream
data into the animator via stdin.

---

## Running the animator

```
cargo run --release -- --stdin [options]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--stdin` | off | Read MXFR frames from stdin |
| `--fps <n>` | 30 | Playback speed for buffered frames |
| `--colormap <name>` | heat | Starting colormap (see list below) |
| `--norm <mode>` | global | Normalization mode (see list below) |
| `--interp <mode>` | nearest | Interpolation mode (see list below) |
| `--title <text>` | none | Text shown in the top-left corner |
| `--bare` | off | Hide colorbar and axis-tick overlays |

Typical invocation:

```bash
python solver.py | cargo run --release -- --stdin --colormap rdbu --norm global
```

---

## MXFR binary frame format

Every frame is a self-contained binary record written to **stdout** (the
producer's stdout, which becomes the animator's stdin).  Fields are
little-endian.

```
Offset    Size       Type      Field
──────    ────       ────────  ─────────────────────────────────────
     0       4       u8[4]     Magic bytes: exactly b"MXFR"
     4       4       u32 LE    Width  W  (pixels, must be ≥ 1)
     8       4       u32 LE    Height H  (pixels, must be ≥ 1)
    12       4       u32 LE    Channels C  (1 = scalar, 3 = RGB)
    16       8       f64 LE    Timestamp (arbitrary physical units; shown in UI)
    24   W×H×C×4    f32 LE[]  Pixel data — W×H×C single-precision floats,
                               row-major, top row first; for C=3 the order is
                               R,G,B per pixel in [0,1] range
```

Total frame size: **24 + W × H × C × 4** bytes.

**Channels:**
- `C = 1` (scalar): single float per pixel, mapped through the colormap LUT after normalization.
- `C = 3` (RGB): three floats per pixel `[R, G, B]` in [0, 1]. Colormap and normalization are bypassed; pixels are displayed directly.

Width and height may change between frames.  The animator re-initialises
the GPU texture on the first frame and accepts only frames whose dimensions
match the first one (mismatched frames are silently dropped by the reader).

### Python helper

```python
import struct, sys, numpy as np

_MAGIC = b'MXFR'

def write_frame(data: np.ndarray, timestamp: float = 0.0) -> None:
    """Write one frame to stdout in MXFR format.

    data      -- 2-D array (H, W) for scalar, or 3-D array (H, W, 3) for RGB.
                 Row 0 is the top of the image as displayed.
                 For RGB, values must be in [0, 1] as float32.
    timestamp -- arbitrary scalar shown in the window title (e.g. sim time).
    """
    if data.ndim == 3:
        h, w, c = data.shape
    else:
        h, w = data.shape
        c = 1
    header = struct.pack('<4sIIId', _MAGIC, w, h, c, timestamp)
    sys.stdout.buffer.write(header)
    sys.stdout.buffer.write(np.ascontiguousarray(data, dtype='<f4').tobytes())
    sys.stdout.buffer.flush()
```

Flush after every frame so the animator receives it immediately rather than
waiting for the OS pipe buffer to fill.

---

## Row / column ordering

The pixel array is **row-major, top-row-first** in display space.

- `data[0, :]` is the top row of the image.
- `data[H-1, :]` is the bottom row.
- Within each row, column 0 is on the left.

If your simulation array uses the physical convention (row 0 = bottom of
domain), flip it before writing:

```python
write_frame(field.T[::-1], timestamp=t)
# .T    — swap (x, y) indexing='ij' to (row, col) order
# [::-1]— flip rows so physical y=0 maps to the last (bottom) display row
```

---

## Pixel values and normalization

Pixel values are **raw floats** in whatever units your simulation uses.
The animator normalises them to [0, 1] before colormap lookup using one of
four modes (choose with `--norm` or press **N** to cycle):

| Mode | Description |
|------|-------------|
| `global` | Scale to the global min/max over all frames received so far. |
| `per-frame` | Scale each frame independently to its own min/max. |
| `percentile` | Like per-frame, but uses the 1st/99th percentile to clip outliers. |
| `fixed` | Use a fixed [0, 1] range — meaningful only if your data is pre-normalised. |

For **signed** fields (e.g. `Hz` out-of-plane magnetic field), use the
`rdbu` colormap with `--norm global` so zero maps to the neutral centre.

For **non-negative** magnitude fields (e.g. `|E|`, `|J|`), use `heat`,
`inferno`, or `viridis`.

---

## Colormaps

| Name | Description |
|------|-------------|
| `heat` | Black → red → orange → yellow → white.  Good for magnitudes. |
| `inferno` | Perceptually uniform dark-to-bright.  Good for magnitudes. |
| `viridis` | Perceptually uniform blue-to-yellow.  Colour-blind friendly. |
| `rdbu` | Diverging red–white–blue.  Best for signed fields centred at zero. |
| `grayscale` | Black → white. |

Press **C** at runtime to cycle through colormaps.

---

## Keyboard controls (runtime)

| Key | Action |
|-----|--------|
| **N** | Cycle normalization mode |
| **I** | Cycle interpolation mode (nearest → bilinear → bicubic) |
| **C** | Cycle colormap |
| **R** | Reset zoom and pan to default |
| **Scroll** | Zoom toward cursor |
| **Drag** | Pan the view |
| **Esc** | Quit |

---

## Diagnostic output from the producer

All human-readable output from your solver must go to **stderr** so that
stdout carries only binary MXFR data:

```python
import sys

def log(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)
```

---

## Minimal working example

```python
#!/usr/bin/env python3
"""Streams a travelling sine wave to wgpu_animator."""
import numpy as np, time, sys, struct

W, H = 256, 256
_MAGIC = b'MXFR'

def write_frame(data, t):
    h, w = data.shape
    sys.stdout.buffer.write(struct.pack('<4sIIId', _MAGIC, w, h, 1, t))
    sys.stdout.buffer.write(np.ascontiguousarray(data, dtype='<f4').tobytes())
    sys.stdout.buffer.flush()

x = np.linspace(0, 2 * np.pi, W)
y = np.linspace(0, 2 * np.pi, H)
X, Y = np.meshgrid(x, y)   # shape (H, W), row 0 = top

for frame in range(300):
    t = frame * 0.05
    field = np.sin(X - t) * np.cos(Y - 0.5 * t)   # range [-1, 1]
    write_frame(field, t)
    time.sleep(1 / 30)
```

Run with:

```bash
python sine_wave.py | ./target/release/wgpu_animator --stdin --colormap rdbu --norm global
```
